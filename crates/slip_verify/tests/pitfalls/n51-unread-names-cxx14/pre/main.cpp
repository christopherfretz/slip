#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace legacy {
void g() throw(decltype(make(0)));
}
