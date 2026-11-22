// std: c++26
// §10.8 Reflectee `Decl` (unsupported, §12): a reflection of this kind as a value.
// The unit is flagged by `ValueKind/Reflection` (`src/value.rs`), which is written
// before any `Reflectee` row is read.
#include "meta.h"
int d;
constexpr auto r = ^^d;
