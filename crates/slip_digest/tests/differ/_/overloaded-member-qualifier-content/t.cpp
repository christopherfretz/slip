#include "side.h"
// §7.4 OverloadedMember `qualifier`, its content: `p->Bc::f(x<T>)` against
// `p->Dc::f(x<T>)` in a template where the access stays overloaded. The
// first candidate is `Bc::f(int)` on both sides and both are qualified;
// the qualifiers name different classes, which are also the naming
// classes (the content is implied by `naming_class`).
struct Bc { int f(int); int f(long); };
struct Dc : Bc {};
Dc *p;
template <class T> T x{};
#ifdef B
template <class T> auto v = p->Bc::f(x<T>);
#else
template <class T> auto v = p->Dc::f(x<T>);
#endif
