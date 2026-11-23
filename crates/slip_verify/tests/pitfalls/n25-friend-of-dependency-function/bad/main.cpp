#include "dep.h"

namespace acme {
class Mine {
  int secret = 1;
  template <class T> friend int peek(const T &);
};
}

int user() { return legacy::peek(acme::Mine()); }
