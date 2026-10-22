-- osascript open_textedit.applescript <file.txt>
-- Waits for the document TextEdit opened on <file.txt>, sets its text to
-- Helvetica Neue 22 pt, brings it to the front and puts the insertion point at
-- the end. Open the file first with `open -a TextEdit <file>` so TextEdit never
-- shows its Open panel, which blocks scripting. A plain-text document styles
-- only text that exists, so the file ends with one space: the typed line takes
-- its font from it. TextEdit's own preferences are not changed.
on run argv
	set thePath to item 1 of argv
	set AppleScript's text item delimiters to "/"
	set theName to last text item of thePath
	set AppleScript's text item delimiters to ""
	tell application "TextEdit"
		set theDoc to missing value
		repeat 40 times
			repeat with candidate in documents
				if name of candidate is theName then set theDoc to contents of candidate
			end repeat
			if theDoc is not missing value then exit repeat
			delay 0.25
		end repeat
		if theDoc is missing value then error "TextEdit did not open " & thePath
		set font of text of theDoc to "Helvetica Neue"
		set size of text of theDoc to 22
		activate
	end tell
	-- Command-Down Arrow: insertion point to the end of the document.
	tell application "System Events" to key code 125 using command down
end run
