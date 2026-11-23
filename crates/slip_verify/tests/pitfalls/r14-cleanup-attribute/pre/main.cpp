int calls = 0;

namespace app {
void release(int *) { calls += 1; }
int f() {
  { int x __attribute__((cleanup(release))) = 0; }
  return calls;
}
}
