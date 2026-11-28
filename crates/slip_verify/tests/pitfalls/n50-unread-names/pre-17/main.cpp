#include "dep.h"

struct X { int b; };

namespace legacy {
template <class T> void ftn(T) {}
struct FN { friend void ftn<>(int (*)[sizeof(X)]); };
}
