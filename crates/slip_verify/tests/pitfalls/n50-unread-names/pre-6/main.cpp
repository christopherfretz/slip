#include "dep.h"

template <class T> int make(T);
template <class T> struct Wr { T m; };

namespace legacy {
#define CAST(w, v) ((_BitInt(w))(v))
int c37() { return (int)CAST(sizeof(make(0)) * 8, 1); }
}
