#include "side.h"
// §5.3 Access `Public`.
#ifdef B
struct C { private: int x; };
#else
struct C { public: int x; };
#endif
