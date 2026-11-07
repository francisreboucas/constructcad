<#
.SYNOPSIS
  Capture the README screenshots from the real app window.

.DESCRIPTION
  Builds the showcase project from examples/showcase.ccmacro (in English it is
  examples/showcase.ccad; other languages get a copy built with that language's
  default names), opens it in the release app once per shot with the views of
  the shot, sizes the window, captures it and closes it. Images go to
  docs/screenshots/<lang>/<shot>.png.

  Needs a display (Windows). Build first:
    cargo build --release -p cc_app -p cc_cli

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts/screenshots.ps1
  powershell -ExecutionPolicy Bypass -File scripts/screenshots.ps1 -Languages en -Shots plumbing,sheet
#>
param(
    [string[]]$Languages = @('en', 'pt-BR'),
    [string[]]$Shots = @(),
    [int]$Width = 1920,
    [int]$Height = 1080,
    # Seconds to wait after the window appears, for the views to render.
    [int]$Settle = 6,
    # Width of the saved images (0 keeps the window size).
    [int]$OutWidth = 0
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$app = Join-Path $root 'target/release/constructcraft.exe'
$cli = Join-Path $root 'target/release/ccad.exe'
$macro = Join-Path $root 'examples/showcase.ccmacro'
foreach ($exe in $app, $cli) {
    if (-not (Test-Path $exe)) { throw "missing ${exe}: run cargo build --release -p cc_app -p cc_cli" }
}

Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Win {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool repaint);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int hh, uint flags);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr h);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);
}
'@
[void][Win]::SetProcessDPIAware()

# Names the script gives, in each language (default names come from `ccad --lang`).
$names = @{
    'pt-BR' = [ordered]@{
        'Showcase house'           = 'Casa de exemplo'
        'Kitchen and bath outlets' = 'Tomadas cozinha e banheiro'
        'Living room'              = 'Sala'
        'Kitchen'                  = 'Cozinha'
        'Bedroom'                  = 'Quarto'
        'Suite'                    = 'Suíte'
        'Bathroom'                 = 'Banheiro'
        'Lighting'                 = 'Iluminação'
        'Outlets'                  = 'Tomadas'
        'Plumbing'                 = 'Hidrossanitário'
        'Electrical'               = 'Elétrico'
        'Board diagram'            = 'Diagrama do quadro'
        'Structure'                = 'Estrutura'
        'Reinforcement'            = 'Armaduras'
        'Analytical model'         = 'Modelo analítico'
        'Section A'                = 'Corte A'
        'Renovation'               = 'Reforma'
        'Partitions'               = 'Divisórias'
        'Wall types'               = 'Tipos de parede'
        '3D North-east'            = '3D Nordeste'
        'Exterior walls'           = 'Paredes externas'
    }
}

# Shots: views opened (the first fills the left pane, the second the right one).
# A view is named by its id in the showcase script, so the list works in every language.
$shotList = [ordered]@{
    'hero'       = @(65, 69)
    'roof-3d'    = @(343)
    'plumbing'   = @(333)
    'electrical' = @(334, 335)
    'structure'  = @(336, 337)
    'graphics'   = @(340, 341)
    'section'    = @(339)
    'sheet'      = @(79)
    'schedule'   = @(78)
}
# `-File` passes a comma-separated list as one string.
$Languages = @($Languages | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
$Shots = @($Shots | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
if ($Shots.Count -gt 0) {
    $selected = [ordered]@{}
    foreach ($s in $Shots) {
        if (-not $shotList.Contains($s)) { throw "unknown shot $s (known: $($shotList.Keys -join ', '))" }
        $selected[$s] = $shotList[$s]
    }
    $shotList = $selected
}

function Build-Project([string]$lang) {
    if ($lang -eq 'en') {
        $out = Join-Path $root 'examples/showcase.ccad'
        $script = $macro
    } else {
        $tmp = Join-Path $root "target/screenshots/$lang"
        New-Item -ItemType Directory -Force $tmp | Out-Null
        $text = [IO.File]::ReadAllText($macro)
        foreach ($k in $names[$lang].Keys) { $text = $text.Replace("`"$k`"", "`"$($names[$lang][$k])`"") }
        $script = Join-Path $tmp 'showcase.ccmacro'
        [IO.File]::WriteAllText($script, $text, (New-Object Text.UTF8Encoding $false))
        $out = Join-Path $tmp 'showcase.ccad'
    }
    & $cli --lang $lang --save-as $out script $script | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "the showcase script failed ($lang)" }
    return $out
}

function View-Names([string]$project, [int[]]$ids) {
    $views = & $cli -p $project run views_list | ConvertFrom-Json
    foreach ($id in $ids) {
        $v = $views | Where-Object { $_.id -eq $id }
        if (-not $v) { throw "no view $id in $project" }
        $v.name
    }
}

function Capture([string]$project, [string]$lang, [string[]]$views, [string]$file) {
    $argList = @("`"$project`"", '--lang', $lang)
    foreach ($v in $views) { $argList += @('--view', "`"$v`"") }
    $env:CONSTRUCTCAD_NO_UPDATE_CHECK = '1'
    $p = Start-Process -FilePath $app -ArgumentList $argList -PassThru
    try {
        $deadline = (Get-Date).AddSeconds(30)
        # The title names the project once it is open.
        while ($p.MainWindowHandle -eq 0 -or $p.MainWindowTitle -notlike 'ConstructCraft *') {
            if ((Get-Date) -gt $deadline) { throw 'the app window did not appear' }
            Start-Sleep -Milliseconds 250
            $p.Refresh()
        }
        Start-Sleep -Seconds 1
        $p.Refresh()
        $h = $p.MainWindowHandle
        # The app may restore a maximized window after it appears: insist.
        $r = New-Object Win+RECT
        for ($i = 0; $i -lt 10; $i++) {
            [void][Win]::ShowWindow($h, 9) # restore
            [void][Win]::MoveWindow($h, 40, 40, $Width, $Height, $true)
            Start-Sleep -Milliseconds 500
            [void][Win]::GetWindowRect($h, [ref]$r)
            if (-not [Win]::IsZoomed($h) -and ($r.Right - $r.Left) -eq $Width -and ($r.Bottom - $r.Top) -eq $Height) { break }
        }
        # Keep it above other windows while it renders and is captured.
        [void][Win]::SetWindowPos($h, [IntPtr](-1), 0, 0, 0, 0, 0x0003)
        [void][Win]::SetForegroundWindow($h)
        Start-Sleep -Seconds $Settle
        # The visible frame, without the invisible resize borders.
        $r = New-Object Win+RECT
        [void][Win]::DwmGetWindowAttribute($h, 9, [ref]$r, 16)
        $w = $r.Right - $r.Left
        $hh = $r.Bottom - $r.Top
        $bmp = New-Object Drawing.Bitmap $w, $hh
        $g = [Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
        $g.Dispose()
        if ($OutWidth -gt 0 -and $OutWidth -lt $w) {
            $oh = [int][Math]::Round($hh * $OutWidth / $w)
            $small = New-Object Drawing.Bitmap $OutWidth, $oh
            $g = [Drawing.Graphics]::FromImage($small)
            $g.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $g.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
            $g.DrawImage($bmp, 0, 0, $OutWidth, $oh)
            $g.Dispose()
            $bmp.Dispose()
            $bmp = $small
        }
        $bmp.Save($file, [Drawing.Imaging.ImageFormat]::Png)
        $bmp.Dispose()
    } finally {
        if (-not $p.HasExited) { $p.Kill() }
        $p.WaitForExit()
    }
}

foreach ($lang in $Languages) {
    $project = Build-Project $lang
    $dir = Join-Path $root "docs/screenshots/$lang"
    New-Item -ItemType Directory -Force $dir | Out-Null
    foreach ($shot in $shotList.Keys) {
        $views = @(View-Names $project $shotList[$shot])
        $file = Join-Path $dir "$shot.png"
        Write-Host "$lang/$shot.png: $($views -join ', ')"
        Capture $project $lang $views $file
    }
}
