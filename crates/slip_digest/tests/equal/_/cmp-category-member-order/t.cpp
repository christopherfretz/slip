#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): the order of the category's values in its class: Kiln finds them by name (kiln_sema `class/compare/mod.rs:165`), and the static members are written sorted.
#ifdef B
namespace std { struct strong_ordering { int v; static const strong_ordering equal, less, greater; };
#else
namespace std { struct strong_ordering { int v; static const strong_ordering less, equal, greater; };
#endif
inline constexpr strong_ordering strong_ordering::less{-1}, strong_ordering::equal{0}, strong_ordering::greater{1}; }
int a = 1, b = 2;
auto v = a <=> b;
