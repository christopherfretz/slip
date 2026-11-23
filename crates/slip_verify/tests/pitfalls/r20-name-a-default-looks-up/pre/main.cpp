const int limit = 4;
namespace app {
const int cap = 8;
struct Box {
  int n = limit;
};
}

int user() { return app::Box().n; }
