// std: c++26
// §10.8 Reflectee `Annotation.0` (unsupported, §12): a reflection of this kind as a value.
// The unit is flagged by `ValueKind/Reflection` (`src/value.rs`), which is written
// before any `Reflectee` row is read.
#include "meta.h"
[[= 1]] int x;
constexpr auto r = std::meta::annotations_of(^^x)[0];
