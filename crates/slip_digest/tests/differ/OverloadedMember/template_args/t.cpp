#include "side.h"
// §7.4 OverloadedMember `template_args`: `p->f<int>(...)` against
// `p->f<long>(...)` in a template where the access stays overloaded.
struct Bt { template <class U> int f(U, int); template <class U> int f(U, long); };
Bt *p;
template <class T> T x{};
#ifdef B
template <class T> auto v = p->f<int>(x<T>, x<T>);
#else
template <class T> auto v = p->f<long>(x<T>, x<T>);
#endif
