// WP-145 item 1 (the role of a string by its field): a pointer to function_name() passed on, whose string object is also the file's (#line names the file as f's pretty name); the object has both roles, so its bytes are written and the rename is seen
#include "srcloc.h"
void take(const char *);
#line 10 "void start()"
void start() { constexpr const char *p = std::source_location::current().function_name(); take(p); }
