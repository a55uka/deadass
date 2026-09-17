Add-Type @'
using System;
using System.Runtime.InteropServices;
public class MemDump {
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool ReadProcessMemory(IntPtr proc, IntPtr addr, byte[] buffer, int size, out int read);
    [DllImport("kernel32.dll")]
    public static extern bool CloseHandle(IntPtr handle);
}
'@

$proc = [MemDump]::OpenProcess(0x0410, $false, 45896)
if ($proc -eq [IntPtr]::Zero) { Write-Host "cannot open process"; exit 1 }

$client = 0x7FFA5C7E0000

# Read the DOS/NT headers to get SizeOfImage
$head = New-Object byte[] 4096
$read = 0
[MemDump]::ReadProcessMemory($proc, [IntPtr]$client, $head, 4096, [ref]$read) | Out-Null
$eLfanew = [BitConverter]::ToInt32($head, 0x3C)
$sizeOfImage = [BitConverter]::ToInt32($head, $eLfanew + 0x50)
Write-Host "client.dll SizeOfImage: $sizeOfImage"

$outPath = "$env:TEMP\client_dump.bin"
$stream = [System.IO.File]::Create($outPath)
$chunk = 4MB
$offset = 0
while ($offset -lt $sizeOfImage) {
    $len = [Math]::Min($chunk, $sizeOfImage - $offset)
    $buf = New-Object byte[] $len
    $got = 0
    if (-not [MemDump]::ReadProcessMemory($proc, [IntPtr]($client + $offset), $buf, $len, [ref]$got)) {
        # Unreadable page (e.g. uncommitted) - write zeros to keep offsets aligned
        $buf = New-Object byte[] $len
        $got = $len
    }
    $stream.Write($buf, 0, $len)
    $offset += $len
}
$stream.Close()
[MemDump]::CloseHandle($proc) | Out-Null
Write-Host "dumped to $outPath ($offset bytes)"
