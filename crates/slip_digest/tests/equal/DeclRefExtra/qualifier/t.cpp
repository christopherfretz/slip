#include "side.h"
// §7.3 DeclRefExtra `qualifier`.
namespace ns { int x; }
using namespace ns;
template<class T> struct A {
#ifdef B
static inline int w = ::ns::x; };
int v = ns::x;
#else
static inline int w = x; };
int v = x;
#endif
