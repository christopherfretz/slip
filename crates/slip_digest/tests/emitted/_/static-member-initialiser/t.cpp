#include "side.h"
// WP-155: the static data member of a class template whose initialiser
// is not a constant (`zz153-a1`'s `Reg<int>::r`): its instantiation is
// emitted whether or not anything refers to it, so its `definition` unit
// is marked, in side A only, where `probe` instantiates it.
extern "C" int puts(const char *);
inline int note() { puts("registered"); return 1; }
template <class T> struct Reg { static int r; };
template <class T> int Reg<T>::r = note();
#ifndef B
template <class U = int> auto probe() { return (void)&Reg<U>::r, 1; }
int x = sizeof(probe());
#else
int x = 4;
#endif
