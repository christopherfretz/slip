#include "side.h"
// §7 ExprNode: `PseudoObjectExpr.a`: the fact differs.
namespace std { struct strong_ordering { int v;
  static const strong_ordering less, equal, greater;
  friend constexpr bool operator==(strong_ordering, int) { return true; }
  friend constexpr bool operator!=(strong_ordering, int) { return true; }
}; inline constexpr strong_ordering strong_ordering::less{-1}, strong_ordering::equal{0},
  strong_ordering::greater{1}; }
struct M { bool operator==(const M &) const; bool operator<(const M &) const; };
#ifdef B
struct X { M k; M m; std::strong_ordering operator<=>(const X &) const = default; };
#else
struct X { M m; M k; std::strong_ordering operator<=>(const X &) const = default; };
#endif
// The use defines the defaulted operator on both sides (its body is the
// resolved unit `definition`, digest-kinds §11.9).
bool r = (X{} <=> X{}) == 0;
