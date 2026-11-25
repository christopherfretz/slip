#include "side.h"
// WP-156 (B3): the member templates of a class made by instantiation are
// listed sorted by key, so declaring them in another order changes no
// unit (as for declared members, digest-kinds §5.5).
template <class T> struct M {
#ifdef B
  template <class U> T b(U);
  template <class U> T a(U);
  template <class U> struct C {};
#else
  template <class U> struct C {};
  template <class U> T a(U);
  template <class U> T b(U);
#endif
};
M<int> m;
