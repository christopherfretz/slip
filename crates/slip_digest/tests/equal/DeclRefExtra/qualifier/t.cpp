#include "side.h"
// §7.3 DeclRefExtra `qualifier`. `u` is a variable template
// specialisation, whose qualifier Kiln keeps (kiln_sema
// templ/actions.rs:1187); `Ct::n` is a qualified non-virtual member
// inside a template pattern.
namespace ns { int x; template<class T> T vt = T(); }
using namespace ns;
struct Am { int m; };
template<class T> struct A {
#ifdef B
static inline int w = ::ns::x; };
int v = ns::x;
int u = ns::vt<int>;
template<class T> struct Ct : Am { int n = Am::m; };
#else
static inline int w = x; };
int v = x;
int u = vt<int>;
template<class T> struct Ct : Am { int n = m; };
#endif
