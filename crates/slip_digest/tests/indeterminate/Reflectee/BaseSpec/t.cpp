// std: c++26
// §10.8 Reflectee `BaseSpec` (unsupported, §12): a reflection of this kind as a value.
// The unit is flagged by `ValueKind/Reflection` (`src/value.rs`), which is written
// before any `Reflectee` row is read.
#include "meta.h"
struct B {};
struct D : B {};
constexpr auto r = std::meta::bases_of(^^D, std::meta::access_context::unchecked())[0];
