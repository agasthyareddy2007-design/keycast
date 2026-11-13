Add-Type -AssemblyName System.Drawing

function Create-KeycastIcon {
    param([string]$OutputPath)

    $sizes = @(16, 24, 32, 48, 64, 128, 256)
    $images = @()

    foreach ($size in $sizes) {
        $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
        $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
        $g.Clear([System.Drawing.Color]::Transparent)

        # Scale factor
        $scale = $size / 256.0

        # Draw rounded rectangle background (tile)
        $pad = 12 * $scale
        $rectW = $size - 2 * $pad
        $rectH = $size - 2 * $pad
        $radius = 42 * $scale

        $path = New-Object System.Drawing.Drawing2D.GraphicsPath
        $path.AddArc($pad, $pad, $radius * 2, $radius * 2, 180, 90)
        $path.AddArc($pad + $rectW - $radius * 2, $pad, $radius * 2, $radius * 2, 270, 90)
        $path.AddArc($pad + $rectW - $radius * 2, $pad + $rectH - $radius * 2, $radius * 2, $radius * 2, 0, 90)
        $path.AddArc($pad, $pad + $rectH - $radius * 2, $radius * 2, $radius * 2, 90, 90)
        $path.CloseFigure()

        # Gradient brush (Deep blue to vibrant cyan-blue)
        $c1 = [System.Drawing.Color]::FromArgb(255, 30, 58, 138)  # #1E3A8A
        $c2 = [System.Drawing.Color]::FromArgb(255, 37, 99, 235)  # #2563EB
        $gradBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
            (New-Object System.Drawing.PointF(0, 0)),
            (New-Object System.Drawing.PointF($size, $size)),
            $c1, $c2
        )
        $g.FillPath($gradBrush, $path)

        # Border highlight
        $borderPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(120, 255, 255, 255), [Math]::Max(1.0, 3 * $scale))
        $g.DrawPath($borderPen, $path)

        # Draw Keyboard Keycap in the center
        # Keycap base:
        $kw = 140 * $scale
        $kh = 100 * $scale
        $kx = ($size - $kw) / 2
        $ky = ($size - $kh) / 2 + 15 * $scale
        $krad = 18 * $scale

        $kpath = New-Object System.Drawing.Drawing2D.GraphicsPath
        $kpath.AddArc($kx, $ky, $krad * 2, $krad * 2, 180, 90)
        $kpath.AddArc($kx + $kw - $krad * 2, $ky, $krad * 2, $krad * 2, 270, 90)
        $kpath.AddArc($kx + $kw - $krad * 2, $ky + $kh - $krad * 2, $krad * 2, $krad * 2, 0, 90)
        $kpath.AddArc($kx, $ky + $kh - $krad * 2, $krad * 2, $krad * 2, 90, 90)
        $kpath.CloseFigure()

        $keyBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(240, 255, 255, 255))
        $g.FillPath($keyBrush, $kpath)

        # Draw "K" on the keycap
        if ($size -ge 32) {
            $fontSize = [float](42 * $scale)
            $font = New-Object System.Drawing.Font("Arial", $fontSize, [System.Drawing.FontStyle]::Bold)
            $textBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 30, 58, 138))
            $sf = New-Object System.Drawing.StringFormat
            $sf.Alignment = [System.Drawing.StringAlignment]::Center
            $sf.LineAlignment = [System.Drawing.StringAlignment]::Center
            $textRect = New-Object System.Drawing.RectangleF($kx, $ky, $kw, $kh)
            $g.DrawString("K", $font, $textBrush, $textRect, $sf)
            $font.Dispose()
            $textBrush.Dispose()
            $sf.Dispose()
        }

        # Draw broadcast waves in top right
        $wavePen1 = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 56, 189, 248), [Math]::Max(1.5, 6 * $scale))
        $wavePen1.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
        $wavePen1.EndCap = [System.Drawing.Drawing2D.LineCap]::Round

        $wavePen2 = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 125, 211, 252), [Math]::Max(1.0, 5 * $scale))
        $wavePen2.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
        $wavePen2.EndCap = [System.Drawing.Drawing2D.LineCap]::Round

        # Arc 1
        $arcX = [float]($kx + ($kw * 0.7))
        $arcY = [float]($ky * 0.9)
        $arc1Center = New-Object System.Drawing.PointF($arcX, $arcY)
        $r1 = [float](28 * $scale)
        $g.DrawArc($wavePen1, [float]($arc1Center.X - $r1), [float]($arc1Center.Y - $r1), [float]($r1 * 2), [float]($r1 * 2), 290, 75)

        # Arc 2
        $r2 = [float](48 * $scale)
        $g.DrawArc($wavePen2, [float]($arc1Center.X - $r2), [float]($arc1Center.Y - $r2), [float]($r2 * 2), [float]($r2 * 2), 290, 75)

        $wavePen1.Dispose()
        $wavePen2.Dispose()
        $borderPen.Dispose()
        $keyBrush.Dispose()
        $gradBrush.Dispose()
        $path.Dispose()
        $kpath.Dispose()
        $g.Dispose()

        $images += $bmp
    }

    # Save to ICO format
    $fs = [System.IO.File]::Create($OutputPath)
    $bw = New-Object System.IO.BinaryWriter($fs)

    # ICONDIR header
    $bw.Write([uint16]0) # Reserved
    $bw.Write([uint16]1) # Type: 1 = ICO
    $bw.Write([uint16]$images.Count)

    # Compute PNG streams
    $pngStreams = @()
    foreach ($img in $images) {
        $ms = New-Object System.IO.MemoryStream
        $img.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
        $pngStreams += $ms
    }

    $offset = 6 + (16 * $images.Count)
    for ($i = 0; $i -lt $images.Count; $i++) {
        $img = $images[$i]
        $stream = $pngStreams[$i]
        $w = if ($img.Width -ge 256) { 0 } else { $img.Width }
        $h = if ($img.Height -ge 256) { 0 } else { $img.Height }

        $bw.Write([byte]$w)
        $bw.Write([byte]$h)
        $bw.Write([byte]0)   # Color palette
        $bw.Write([byte]0)   # Reserved
        $bw.Write([uint16]1) # Color planes
        $bw.Write([uint16]32)# Bits per pixel
        $bw.Write([uint32]$stream.Length) # Image size
        $bw.Write([uint32]$offset)        # Offset

        $offset += $stream.Length
    }

    # Write image data
    for ($i = 0; $i -lt $images.Count; $i++) {
        $data = $pngStreams[$i].ToArray()
        $bw.Write($data)
        $pngStreams[$i].Dispose()
        $images[$i].Dispose()
    }

    $bw.Flush()
    $fs.Close()
    Write-Host "Created icon at $OutputPath"
}

New-Item -ItemType Directory -Force -Path "C:\Users\Public\Music\project\assets" | Out-Null
Create-KeycastIcon -OutputPath "C:\Users\Public\Music\project\assets\keycast.ico"
