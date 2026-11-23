template <class T> int t() {
  extern int renamed();
  return renamed();
}
