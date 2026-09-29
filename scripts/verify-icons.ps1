param(
    [Parameter(Mandatory=$true)][string]$SourceIcon,
    [Parameter(Mandatory=$true)][string[]]$Targets,
    [Parameter(Mandatory=$true)][string]$OutputDirectory
)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class EasySwitchIconResources {
    public delegate bool ResourceName(IntPtr module, IntPtr type, IntPtr name, IntPtr param);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr LoadLibraryEx(string file, IntPtr reserved, uint flags);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool EnumResourceNames(IntPtr module, IntPtr type, ResourceName callback, IntPtr param);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] static extern IntPtr FindResource(IntPtr module, IntPtr name, IntPtr type);
    [DllImport("kernel32.dll")] static extern IntPtr LoadResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32.dll")] static extern uint SizeofResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32.dll")] static extern IntPtr LockResource(IntPtr resource);
    [DllImport("kernel32.dll")] static extern bool FreeLibrary(IntPtr module);
    public static byte[][] Read(string file) {
        var module=LoadLibraryEx(file, IntPtr.Zero, 2);
        if(module==IntPtr.Zero) throw new System.ComponentModel.Win32Exception();
        var images=new List<byte[]>();
        try {
            ResourceName callback=(m,t,n,p)=>{
                var resource=FindResource(m,n,t);
                var bytes=new byte[SizeofResource(m,resource)];
                Marshal.Copy(LockResource(LoadResource(m,resource)),bytes,0,bytes.Length);
                images.Add(bytes); return true;
            };
            EnumResourceNames(module,new IntPtr(3),callback,IntPtr.Zero);
        } finally { FreeLibrary(module); }
        return images.ToArray();
    }
}
'@
function Hash-Bytes([byte[]]$Bytes) {
    $sha=[Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($Bytes))).Replace('-','').ToLowerInvariant() }
    finally { $sha.Dispose() }
}
$source=(Resolve-Path -LiteralPath $SourceIcon).Path
$bytes=[IO.File]::ReadAllBytes($source)
if([BitConverter]::ToUInt16($bytes,2) -ne 1){throw 'Source must be an ICO file.'}
$count=[BitConverter]::ToUInt16($bytes,4)
$expected=@(for($i=0;$i -lt $count;$i++) {
    $offset=6+16*$i
    $length=[BitConverter]::ToUInt32($bytes,$offset+8)
    $start=[BitConverter]::ToUInt32($bytes,$offset+12)
    $data=New-Object byte[] $length
    [Array]::Copy($bytes,$start,$data,0,$length)
    [pscustomobject]@{width=$(if($bytes[$offset] -eq 0){256}else{[int]$bytes[$offset]});sha256=(Hash-Bytes $data)}
})
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$results=@(foreach($target in $Targets) {
    $file=(Resolve-Path -LiteralPath $target).Path
    $actual=@([EasySwitchIconResources]::Read($file) | ForEach-Object {Hash-Bytes $_})
    $missing=@($expected | Where-Object {$_.sha256 -notin $actual})
    $preview=Join-Path $OutputDirectory (([IO.Path]::GetFileNameWithoutExtension($file))+'.png')
    $icon=[Drawing.Icon]::ExtractAssociatedIcon($file)
    if($null -eq $icon){throw "No shell icon: $file"}
    $bitmap=$icon.ToBitmap()
    try {$bitmap.Save($preview,[Drawing.Imaging.ImageFormat]::Png)} finally {$bitmap.Dispose();$icon.Dispose()}
    [pscustomobject]@{file=$file;sha256=(Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant();iconResourceCount=$actual.Count;expectedSizes=@($expected.width);missingSizes=@($missing | ForEach-Object {$_.width});passed=($missing.Count -eq 0 -and $actual.Count -eq $expected.Count);preview=$preview}
})
$report=[pscustomobject]@{source=$source;sourceSha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant();results=$results;passed=(@($results | Where-Object {!$_.passed}).Count -eq 0)}
$report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.json') -Encoding UTF8
$report | ConvertTo-Json -Depth 6
if(!$report.passed){exit 1}
exit 0
