#include "side.h"
// §5.4 FnSpec `0-exc-spec-computed`: a defaulted constructor whose specification is computed against one written as what it computes to.
struct M { M() noexcept(false); };
#ifdef B
struct S { M m; S() noexcept(false) = default; };
#else
struct S { M m; S() = default; };
#endif
bool b = noexcept(S());
