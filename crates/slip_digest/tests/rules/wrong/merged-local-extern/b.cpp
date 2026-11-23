namespace legacy {
int hidden();
}
namespace acme {
template <class T> int t() {
  extern int hidden();
  return hidden();
}
}
