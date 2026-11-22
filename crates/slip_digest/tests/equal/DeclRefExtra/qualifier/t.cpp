#include "side.h"
// §7.3 DeclRefExtra `qualifier`. `u` is a variable template
// specialisation, whose qualifier Kiln keeps (kiln_sema
// templ/actions.rs:1187).
namespace ns { int x; template<class T> T vt = T(); }
using namespace ns;
template<class T> struct A {
#ifdef B
static inline int w = ::ns::x; };
int v = ns::x;
int u = ns::vt<int>;
#else
static inline int w = x; };
int v = x;
int u = vt<int>;
#endif
