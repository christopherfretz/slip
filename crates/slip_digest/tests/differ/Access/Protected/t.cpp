#include "side.h"
// §5.3 Access `Protected`.
#ifdef B
struct C { private: int x; };
#else
struct C { protected: int x; };
#endif
