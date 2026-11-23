#include "user_types.h"
int user() { return sizeof(legacy::UserData) + sizeof(legacy::Dep); }
