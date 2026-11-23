// §10.2, variables named `old_name` (in another namespace, and a local):
// untouched.
namespace lib {
int old_name(int);
}
namespace other {
int old_name = 0;
}
int use() {
  int old_name = 1;
  return lib::old_name(old_name) + other::old_name;
}
