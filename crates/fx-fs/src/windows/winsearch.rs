//! Windows Search (T6.8, D-068): files whose content the Windows index says
//! holds a text, through its OLE DB provider (`Search.CollatorDSO`), run in
//! the hidden PowerShell. Covers only the places Windows indexes (usually
//! the user's folders); fast because Windows already did the reading.

use std::io;
use std::path::{Path, PathBuf};

use super::powershell::{list_file, ps_quote, run_powershell};

/// Up to `limit` paths of files containing `text`, under `scope` when
/// given (else everything indexed), calling `found` for each.
pub fn search_content(
    text: &str,
    scope: Option<&Path>,
    limit: usize,
    found: &mut dyn FnMut(PathBuf),
) -> io::Result<()> {
    let mut lines = vec![text.to_owned()];
    if let Some(s) = scope {
        // SCOPE takes a file: URL with forward slashes.
        lines.push(format!("file:{}", s.to_string_lossy().replace('\\', "/")));
    }
    let list = list_file(&lines)?;
    let script = format!(
        "$ErrorActionPreference='Stop'\n\
         [Console]::OutputEncoding=[Text.Encoding]::UTF8\n\
         $in=[IO.File]::ReadAllLines({list},[Text.Encoding]::UTF8)\n\
         $t=$in[0].Replace(\"'\",\"''\").Replace('\"','\"\"')\n\
         $where=\"CONTAINS(*,'\"\"$t\"\"')\"\n\
         if($in.Length -gt 1 -and $in[1] -ne '') {{ $s=$in[1].Replace(\"'\",\"''\"); $where=\"SCOPE='$s' AND $where\" }}\n\
         $c=New-Object -ComObject ADODB.Connection\n\
         $c.Open(\"Provider=Search.CollatorDSO;Extended Properties='Application=Windows';\")\n\
         $r=New-Object -ComObject ADODB.Recordset\n\
         $r.Open(\"SELECT TOP {limit} System.ItemPathDisplay FROM SystemIndex WHERE $where\",$c)\n\
         while(-not $r.EOF) {{ [Console]::Out.WriteLine($r.Fields.Item(0).Value); $r.MoveNext() }}\n\
         $r.Close(); $c.Close()\n",
        list = ps_quote(&list.0),
    );
    run_powershell(&script, &mut |l| {
        if !l.is_empty() {
            found(PathBuf::from(l));
        }
    })
}

#[cfg(test)]
mod tests {
    /// Asks the real index for a word that is surely not in it: no error
    /// (or the service is off, which is an error, not a hang).
    #[test]
    fn query_runs_without_a_window() {
        let mut n = 0;
        let r = super::search_content("fxpalavraquenaoexiste9137", None, 10, &mut |_| n += 1);
        if let Err(e) = &r {
            println!("Windows Search unavailable here: {e}");
        }
        assert_eq!(n, 0);
    }
}
