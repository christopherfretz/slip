#include "side.h"
// digest §14 (WP-045 review probe): the overload ADL finds at the point
// of instantiation of S<N::A>::x (after h(), which uses it) differs: the
// instantiation differs, and only there.
namespace N { struct A {}; char g(A, int); }
#ifdef B
namespace N { long g(A, long); }
#endif
template <class T> struct S { static constexpr int x = sizeof(g(T(), 0L)); };
int h() { return S<N::A>::x; }
#ifndef B
namespace N { long g(A, long); }
#endif
