#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace acme {
void g() throw(decltype(legacy::make(0)));
}
