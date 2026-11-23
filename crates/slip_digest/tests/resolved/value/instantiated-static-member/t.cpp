#include "side.h"
// digest-kinds §11.3: Kiln evaluates the initialiser of a static data
// member of an instantiation when it instantiates it, which a use does.
template <class T> struct A { static constexpr int v = sizeof(T); };
A<int> a;
#ifdef B
int h() { int x[A<int>::v]; return sizeof x; }
#endif
