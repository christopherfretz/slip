int f() {
  struct Local { int v = 1; };
  return Local().v;
}
