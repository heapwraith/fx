# Quick Look fixture (T1.22): %TEMP%\fx-fixtures\preview with a photo-like
# JPEG and a PNG of about 10 MB each, plus a text file. Created once; called
# by `fx-bench features`.
$ErrorActionPreference = 'Stop'
$d = Join-Path $env:TEMP 'fx-fixtures\preview'
if (Test-Path (Join-Path $d 'z-texto.txt')) { return }
New-Item -ItemType Directory -Force $d | Out-Null
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
public static class Px { public static byte[] Fill(int w, int h, int stride, int noise) {
  var buf = new byte[stride * h]; var rnd = new System.Random(7);
  for (int y = 0; y < h; y++) for (int x = 0; x < w; x++) { int o = y * stride + x * 3, n = rnd.Next(noise);
    buf[o] = (byte)((x * 255 / w + n) % 256); buf[o+1] = (byte)((y * 255 / h + n) % 256); buf[o+2] = (byte)(((x + y) * 128 / w + n) % 256); }
  return buf; } }
"@
function Img([int]$w, [int]$h, [string]$path, [int]$noise) {
    $b = New-Object System.Drawing.Bitmap $w, $h, ([System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $data = $b.LockBits((New-Object System.Drawing.Rectangle 0, 0, $w, $h), 'WriteOnly', $b.PixelFormat)
    $buf = [Px]::Fill($w, $h, $data.Stride, $noise)
    [System.Runtime.InteropServices.Marshal]::Copy($buf, 0, $data.Scan0, $buf.Length); $b.UnlockBits($data)
    if ($path.EndsWith('.png')) { $b.Save($path, [System.Drawing.Imaging.ImageFormat]::Png) } else {
        $codec = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object MimeType -eq 'image/jpeg'
        $ep = New-Object System.Drawing.Imaging.EncoderParameters 1
        $ep.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter ([System.Drawing.Imaging.Encoder]::Quality), ([long]95)
        $b.Save($path, $codec, $ep)
    }
    $b.Dispose()
}
Img 4000 3000 (Join-Path $d 'a-foto.jpg') 90
Img 2200 1650 (Join-Path $d 'b-imagem.png') 200
Set-Content (Join-Path $d 'z-texto.txt') ((1..20000 | ForEach-Object { "line $_ of a large text file" }) -join "`r`n")
Get-ChildItem $d | ForEach-Object { "{0} {1:N1} MB" -f $_.Name, ($_.Length / 1MB) }
