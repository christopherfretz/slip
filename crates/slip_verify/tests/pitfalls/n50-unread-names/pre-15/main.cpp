#include "dep.h"

struct X { int b; };

namespace legacy {
_BitInt(sizeof(X) * 8) bi = 0;
}
