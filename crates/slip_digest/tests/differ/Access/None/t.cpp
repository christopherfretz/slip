#include "side.h"
// §5.3 Access `None`: a namespace member against a class member is another key; here a static member against a namespace variable seen through `using`.
#ifdef B
namespace n { int x; } int y;
#else
namespace n { int x; }
#endif
