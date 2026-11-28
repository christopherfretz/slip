#include "side.h"
#include "other.h"
#ifdef B
namespace acme { using other::v; int g() { return v; } }
#else
namespace legacy { using other::v; int g() { return v; } }
#endif
