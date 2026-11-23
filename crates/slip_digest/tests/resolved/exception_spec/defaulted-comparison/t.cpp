#include "side.h"
// digest-kinds §11.2 rule 1: a comparison operator explicitly defaulted
// on its first declaration (class/compare/default.rs defers it).
struct S {
  int x;
  bool operator==(const S &) const = default;
};
#ifdef B
bool h() { return noexcept(S() == S()); }
#endif
