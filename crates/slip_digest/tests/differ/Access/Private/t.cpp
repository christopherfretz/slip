#include "side.h"
// §5.3 Access `Private`.
#ifdef B
struct C { public: int x; };
#else
struct C { private: int x; };
#endif
