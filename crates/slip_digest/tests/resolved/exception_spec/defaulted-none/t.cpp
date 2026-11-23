#include "side.h"
// digest-kinds §11.2 rule 1: explicitly defaulted on its first
// declaration, no specification written; Kiln computes it when needed.
struct M { M() noexcept(false); };
struct S { M m; S() = default; };
#ifdef B
bool h() { return noexcept(S()); }
#endif
