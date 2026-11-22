#include "side.h"
// §5.4 FnSpec `0-trivial`: a defaulted copy constructor is trivial when its member's is. The member's type is the same on both sides and its copy constructor constexpr and noexcept on both, so `C`'s copy constructor differs only in the bit.
#ifdef B
struct M { M() = default; M(const M&) = default; };
struct C { M m; C(const C&) = default; };
#else
struct M { M() = default; constexpr M(const M&) noexcept {} };
struct C { M m; C(const C&) = default; };
#endif
