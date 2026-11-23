// WP-104 (digest §9), review G1 control: a passed-on std::source_location moved down a line in the same file is equal (its line is dropped, its file is kept)
#include "srcloc.h"
void log(std::source_location);
void f(int) {
  log(std::source_location::current());
}
