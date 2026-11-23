int wf(int);
namespace n { using ::wf; }
namespace __attribute__((visibility("default"))) n { using ::wf; }
int u() { return n::wf(1); }
