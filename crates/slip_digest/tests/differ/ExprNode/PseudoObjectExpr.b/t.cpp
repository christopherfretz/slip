#include "side.h"
// §7 ExprNode: `PseudoObjectExpr.b`: the synthesized form on side A only.
namespace std { struct strong_ordering { int v;
  static const strong_ordering less, equal, greater;
  friend constexpr bool operator==(strong_ordering, int) { return true; }
  friend constexpr bool operator!=(strong_ordering, int) { return true; }
}; inline constexpr strong_ordering strong_ordering::less{-1}, strong_ordering::equal{0},
  strong_ordering::greater{1}; }
struct M { bool operator==(const M &) const; bool operator<(const M &) const; };
#ifdef B
struct X { int m; std::strong_ordering operator<=>(const X &) const = default; };
#else
struct X { M m; std::strong_ordering operator<=>(const X &) const = default; };
#endif
