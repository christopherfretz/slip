#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace acme {
void d10() { Wr<int> wv{0}; wv.~decltype(Wr<int>{legacy::make(0)})(); }
}
