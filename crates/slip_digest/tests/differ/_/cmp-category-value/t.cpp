#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): another value for `less`: its unit, and a constant `<=>` (the Oven reads `ints`).
#ifdef B
namespace std { struct strong_ordering { int v; static const strong_ordering less, equal, greater; };
inline constexpr strong_ordering strong_ordering::less{-2}, strong_ordering::equal{0}, strong_ordering::greater{1}; }
#else
namespace std { struct strong_ordering { int v; static const strong_ordering less, equal, greater; };
inline constexpr strong_ordering strong_ordering::less{-1}, strong_ordering::equal{0}, strong_ordering::greater{1}; }
#endif
int a = 1, b = 2;
auto v = a <=> b;
constexpr auto w = 1 <=> 2;
