#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace acme {
#define SZ(w) sizeof(_BitInt(w))
unsigned long c38() { return SZ(sizeof(make(0)) * 8); }
}
