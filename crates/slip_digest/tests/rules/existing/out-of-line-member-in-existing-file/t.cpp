#include "side.h"
#ifdef B
namespace acme { struct C { int m(); }; }
#else
namespace legacy { struct C { int m(); }; }
#endif
#include "existing_part.h"
