#include "dep.h"

namespace legacy {
template <class T> T make(int) { return T(); }
}

#define MAKE(T) legacy::make<T>(0)

int user() { return MAKE(int); }
