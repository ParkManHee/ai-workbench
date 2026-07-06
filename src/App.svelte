<script lang="ts">
  import { call } from "./lib/api";

  let pingResult = $state("(loading...)");

  call<string>("ping")
    .then((result) => {
      pingResult = result;
    })
    .catch((err) => {
      pingResult = `error: ${err}`;
    });
</script>

<main>
  <h1>ai-workbench</h1>
  <p>ping result: <strong>{pingResult}</strong></p>
</main>
