#include "own.h"
#ifdef POST
int user(acme::Mine* m) { return m->v; }
#else
int user(legacy::Mine* m) { return m->v; }
#endif
