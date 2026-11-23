#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a constant object's image follows the layout (digest §7).
#ifdef B
#pragma pack(push, 1)
#endif
struct S { char c; int i; };
#ifdef B
#pragma pack(pop)
#endif
constexpr S s{1, 2};
