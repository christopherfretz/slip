#define POST 1
#include "user_types.h"
int user() { return sizeof(acme::UserData) + sizeof(legacy::Dep); }
