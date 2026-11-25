#include "side.h"
template <class...> using void_t = void;
struct Base { typedef long t; };
namespace n { struct S : Base {
#ifdef B
  typedef int u;
#else
  typedef int t;
#endif
}; }
template <class T> void_t<typename T::t> *f(T);
void *p = f(n::S());
template <class T> struct C { void_t<typename T::t> *m; };
C<n::S> c;
