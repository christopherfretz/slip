#include "side.h"
// digest-kinds §3.3 (`function` row, the befriended discriminator): a
// member function and a friend of one name and parameter types in a class
// template are two units; defining the friend changes the friend's unit
// only (a body's statements are pending, its presence is not).
template <class T> struct A {
  void g(int);
#ifdef B
  friend void g(int) {}
#else
  friend void g(int);
#endif
};
