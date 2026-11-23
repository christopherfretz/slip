int f(int) {
  char name[sizeof(__func__)];
  return sizeof(name);
}
