#include "side.h"
// digest-kinds §11.1 property 5: a virtual destructor's operator delete is
// the global one against the class's own.
struct S {
  virtual ~S();
#ifdef B
  void operator delete(void *);
#endif
};
S::~S() {}
