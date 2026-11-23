#include "dep.h"

namespace acme {
class Mine {
  int secret = 1;
  template <class T> friend auto legacy::peek(const T &t) -> decltype(t.secret);
};
}

int user() { return legacy::peek(acme::Mine()); }
