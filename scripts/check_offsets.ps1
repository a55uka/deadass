Add-Type @'
using System;
using System.Runtime.InteropServices;
public class MemRead {
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
    [DllImport("kernel32.dll", SetLastError=true)]
    public static extern bool ReadProcessMemory(IntPtr proc, IntPtr addr, byte[] buffer, int size, out int read);
    [DllImport("kernel32.dll")]
    public static extern bool CloseHandle(IntPtr handle);
}
'@

$proc = [MemRead]::OpenProcess(0x0410, $false, 45896)  # VM_READ | QUERY_INFORMATION
if ($proc -eq [IntPtr]::Zero) { Write-Host "cannot open process"; exit 1 }

function Read-U64($addr) {
    $buf = New-Object byte[] 8
    $read = 0
    if (-not [MemRead]::ReadProcessMemory($proc, [IntPtr]$addr, $buf, 8, [ref]$read)) { return $null }
    return [BitConverter]::ToUInt64($buf, 0)
}

$client = 0x7FFA5C7E0000
$pawnPtr   = Read-U64 ($client + 0x2E76FE8)
$entSysPtr = Read-U64 ($client + 0x318A790)
Write-Host ("local pawn global  -> 0x{0:X}" -f $pawnPtr)
Write-Host ("entity system ptr  -> 0x{0:X}" -f $entSysPtr)

if ($pawnPtr -ne 0) {
    $health    = Read-U64 ($pawnPtr + 0x2D0)
    $buf = New-Object byte[] 4
    $read = 0
    [MemRead]::ReadProcessMemory($proc, [IntPtr]($pawnPtr + 0x2D0), $buf, 4, [ref]$read) | Out-Null
    $hp = [BitConverter]::ToInt32($buf, 0)
    $buf2 = New-Object byte[] 4
    [MemRead]::ReadProcessMemory($proc, [IntPtr]($pawnPtr + 0x2D8), $buf2, 4, [ref]$read) | Out-Null
    $life = $buf2[0]
    $buf3 = New-Object byte[] 8
    [MemRead]::ReadProcessMemory($proc, [IntPtr]($pawnPtr + 0x1A58), $buf3, 8, [ref]$read) | Out-Null
    $count = [BitConverter]::ToUInt32($buf3, 0)
    $dataPtr = [BitConverter]::ToUInt64($buf3, 4)  # note: not aligned read of the pointer half
    Write-Host ("pawn {0:X}: health={1} life_state={2} abilities_count={3}" -f $pawnPtr, $hp, $life, $count)
}

[MemRead]::CloseHandle($proc) | Out-Null
