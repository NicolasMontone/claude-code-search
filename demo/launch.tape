# Launch video for ccs. Run via demo/record.sh (it builds the fake home first).
# Everything shown is synthetic: HOME points at demo/out/home and `claude`
# is demo/bin/claude, a stand-in that just shows what was resumed.

Output demo/out/raw.mp4

Set Shell "bash"
Set FontFamily "Geist Mono"
Set FontSize 21
Set LineHeight 1.25
Set Width 1920
Set Height 1080
Set Padding 40
Set Theme "Catppuccin Mocha"
Set WindowBar Colorful
Set WindowBarSize 44
Set BorderRadius 12
Set Margin 36
Set MarginFill "#1e1e2e"
Set TypingSpeed 75ms
Set CursorBlink false

Hide
Type "source demo/shellrc"
Enter
Sleep 800ms
Show

Sleep 1s

# Open the picker: last-used first, current project highlighted.
Type "ccs"
Sleep 500ms
Enter
Sleep 3s
Down@450ms 3
Sleep 1s
Up@300ms 3
Sleep 600ms

# Scope to this project and back.
Tab
Sleep 2s
Tab
Sleep 1s

# Search across titles, paths, branches and every prompt.
Type@110ms "rate limit"
Sleep 2.5s
Down
Sleep 2s
Up
Sleep 1s

# Find the one you were in, hit enter: claude resumes in the right dir.
Ctrl+U
Sleep 500ms
Type@110ms "flaky"
Sleep 1.8s
Enter
Sleep 4s
Ctrl+C
Sleep 800ms

# Or skip the picker entirely.
Type "ccs -1 onboarding"
Sleep 600ms
Enter
Sleep 3.5s
Ctrl+C
Sleep 1.5s
