#include "dep.h"

struct X { int b; };

namespace acme {
_BitInt(sizeof(legacy::X) * 8) bi = 0;
}
