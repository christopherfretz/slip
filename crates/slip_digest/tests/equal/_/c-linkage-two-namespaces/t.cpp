#include "side.h"
// digest-kinds §5.2: one C function declared in two namespaces is one unit.
namespace a { extern "C" void cf(int); }
namespace b {
#ifdef B
extern "C" void cf(int);
#endif
}
