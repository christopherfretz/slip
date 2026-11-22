#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a c linkage namespace.
#ifdef B
namespace b { extern "C" void f(); }
#else
namespace a { extern "C" void f(); }
#endif
