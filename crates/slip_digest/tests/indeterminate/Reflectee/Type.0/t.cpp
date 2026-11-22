// std: c++26
// §10.8 Reflectee `Type.0` (unsupported, §12): a reflection of this kind as a value.
// The unit is flagged by `ValueKind/Reflection` (`src/value.rs`), which is written
// before any `Reflectee` row is read.
#include "meta.h"
constexpr auto r = ^^int;
