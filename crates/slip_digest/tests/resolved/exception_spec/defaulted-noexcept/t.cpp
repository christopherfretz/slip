#include "side.h"
// digest-kinds §11.2 rule 1, `noexcept`: once resolved Kiln's type does
// not say whether a specification was written, so every function
// explicitly defaulted on its first declaration is of the class; the
// written `noexcept` is the resolved unit's, on both sides.
struct M { M() noexcept(false); };
struct S { M m; S() noexcept = default; };
#ifdef B
bool h() { return noexcept(S()); }
#endif
