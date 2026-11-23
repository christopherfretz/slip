int calls = 0;

namespace app {
void drop(int *) { calls += 1; }
int f() {
  { int x __attribute__((cleanup(drop))) = 0; }
  return calls;
}
}
