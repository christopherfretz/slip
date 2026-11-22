#include "side.h"
// §5.4 FnSpec `0-trivial`: a defaulted copy constructor is trivial unless a member's is not; both members' copy constructors are noexcept.
#ifdef B
struct M { M(const M&) noexcept; };
struct N {};
struct C { N m; C(const C&) = default; };
#else
struct M { M(const M&) noexcept; };
struct N {};
struct C { M m; C(const C&) = default; };
#endif
