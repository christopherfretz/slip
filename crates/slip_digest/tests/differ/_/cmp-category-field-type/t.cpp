#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): another type for the category's data member: the member and every value.
#ifdef B
namespace std { struct strong_ordering { signed char v;
#else
namespace std { struct strong_ordering { int v;
#endif
static const strong_ordering less, equal, greater; };
inline constexpr strong_ordering strong_ordering::less{-1}, strong_ordering::equal{0}, strong_ordering::greater{1}; }
int a = 1, b = 2;
auto v = a <=> b;
