namespace legacy {
int hidden();
template <class T> int t() {
  extern int hidden();
  return hidden();
}
}
