#include "side.h"
// digest-kinds §3.3 (`function` row, the befriended discriminator): a
// member function and a friend of one name and parameter types in a class
// template are two units, keyed apart.
template <class T> struct A { void g(int); friend void g(int); };
