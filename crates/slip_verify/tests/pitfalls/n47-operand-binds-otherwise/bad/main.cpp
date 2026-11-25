#include "dep.h"

template <class T> int make(T);

namespace acme {
decltype(make(0)) v = 0;
}
