#include "side.h"
// §5.3 DeclMisc `0-access`.
#ifdef B
struct C { private: int x; };
#else
struct C { public: int x; };
#endif
