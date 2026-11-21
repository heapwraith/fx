# fx patch: precompiles gpui's HLSL shaders to DXBC with d3dcompiler_47.dll (ships
# with Windows 10/11), so neither fxc.exe nor the Windows SDK is needed and the app
# does not compile shaders at startup (~250 ms). Run on Windows (from WSL:
# `powershell.exe -File third_party/gpui/compile-shaders.ps1`) after changing any
# .hlsl file, and commit the output in shaders-dxbc/.
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

[ComImport, Guid("8BA5FB08-5195-40e2-AC58-0D989C3A0102"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface ID3DBlob {
    [PreserveSig] IntPtr GetBufferPointer();
    [PreserveSig] UIntPtr GetBufferSize();
}

public static class Fxc {
    [DllImport("d3dcompiler_47.dll", CharSet = CharSet.Ansi)]
    static extern int D3DCompile(byte[] src, UIntPtr srcSize, string name, IntPtr defines,
        IntPtr include, string entry, string target, uint flags1, uint flags2,
        out ID3DBlob code, out ID3DBlob errors);

    static byte[] Bytes(ID3DBlob blob) {
        var size = (int)blob.GetBufferSize().ToUInt64();
        var bytes = new byte[size];
        Marshal.Copy(blob.GetBufferPointer(), bytes, 0, size);
        return bytes;
    }

    public static byte[] Compile(string source, string entry, string target) {
        const uint OPTIMIZATION_LEVEL3 = 1u << 15;
        var src = System.Text.Encoding.UTF8.GetBytes(source);
        ID3DBlob code, errors;
        int hr = D3DCompile(src, (UIntPtr)src.Length, null, IntPtr.Zero, IntPtr.Zero,
            entry, target, OPTIMIZATION_LEVEL3, 0, out code, out errors);
        if (hr < 0) {
            var msg = errors != null ? System.Text.Encoding.ASCII.GetString(Bytes(errors)) : "";
            throw new Exception(entry + ": D3DCompile failed 0x" + hr.ToString("X8") + " " + msg);
        }
        return Bytes(code);
    }
}
"@

$dir = Join-Path $PSScriptRoot 'src\platform\windows'
$out = Join-Path $PSScriptRoot 'shaders-dxbc'
New-Item -ItemType Directory -Force -Path $out | Out-Null
$alpha = [IO.File]::ReadAllText((Join-Path $dir 'alpha_correction.hlsl'))
function Load($file) {
    ([IO.File]::ReadAllText((Join-Path $dir $file))).Replace('#include "alpha_correction.hlsl"', $alpha)
}
$shaders = Load 'shaders.hlsl'
$emoji = Load 'color_text_raster.hlsl'

$modules = @('quad', 'shadow', 'path_rasterization', 'path_sprite', 'underline',
    'monochrome_sprite', 'polychrome_sprite', 'emoji_rasterization')
foreach ($m in $modules) {
    $src = if ($m -eq 'emoji_rasterization') { $emoji } else { $shaders }
    foreach ($stage in @(@('vertex', 'vs_4_1'), @('fragment', 'ps_4_1'))) {
        $bytes = [Fxc]::Compile($src, "${m}_$($stage[0])", $stage[1])
        [IO.File]::WriteAllBytes((Join-Path $out "${m}_$($stage[0]).dxbc"), $bytes)
        Write-Output ("{0,-34} {1,6} bytes" -f "${m}_$($stage[0]).dxbc", $bytes.Length)
    }
}
