#include "dep.h"

struct X { int b; };

namespace acme {
_BitInt(sizeof(X) * 8) bi = 0;
}
