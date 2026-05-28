import @types { Box as ImportedBox }
import @types { Maybe as ImportedMaybe }

StringBox as ImportedBox of String

item StringBox = ImportedBox("text")
raw ImportedBox of String = item

maybe ImportedMaybe of Int = ImportedMaybe::Some(7)

io(raw.value)

[: generic import alias concrete alias]
