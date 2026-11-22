#include "side.h"
// §7 ExprNode: `PseudoObjectExpr.a`: the fact differs.
namespace std { struct strong_ordering { int v;
  static const strong_ordering less, equal, greater;
  friend constexpr bool operator==(strong_ordering, int) { return true; }
  friend constexpr bool operator!=(strong_ordering, int) { return true; }
}; inline constexpr strong_ordering strong_ordering::less{-1}, strong_ordering::equal{0},
  strong_ordering::greater{1}; }
#ifdef B
struct X { int m; int n; auto operator<=>(const X &) const = default; };
#else
struct X { int n; int m; auto operator<=>(const X &) const = default; };
#endif
