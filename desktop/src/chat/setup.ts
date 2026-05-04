// Lazy initialization of the pi-web-ui storage + agent, mirroring the
// pi-web-ui example but stripped of features Meridian doesn't need:
//  - no custom-providers store (we don't ship Ollama UX yet)
//  - no session URL routing (the chat is a global sidebar, not a page)
//  - no theme toggle (Meridian's titlebar already has one)
//
// Returns a singleton agent + ChatPanel so the conversation state persists
// across navigation between Tasks / Pages / Live / etc.

import { Agent } from "@mariozechner/pi-agent-core";
import { getModel } from "@mariozechner/pi-ai";
import {
  AppStorage,
  ChatPanel,
  CustomProvidersStore,
  IndexedDBStorageBackend,
  ProviderKeysStore,
  SessionsStore,
  SettingsStore,
  setAppStorage,
  defaultConvertToLlm,
  ApiKeyPromptDialog,
} from "@mariozechner/pi-web-ui";

import { defaultPagesTools, PAGES_SYSTEM_PROMPT } from "./tools";

interface Bundle {
  agent: Agent;
  chatPanel: ChatPanel;
}

let pending: Promise<Bundle> | null = null;

export function ensureChat(): Promise<Bundle> {
  if (!pending) pending = build();
  return pending;
}

async function build(): Promise<Bundle> {
  const settings = new SettingsStore();
  const providerKeys = new ProviderKeysStore();
  const sessions = new SessionsStore();
  const customProviders = new CustomProvidersStore();

  const backend = new IndexedDBStorageBackend({
    dbName: "meridian-chat",
    version: 1,
    stores: [
      settings.getConfig(),
      SessionsStore.getMetadataConfig(),
      providerKeys.getConfig(),
      customProviders.getConfig(),
      sessions.getConfig(),
    ],
  });

  settings.setBackend(backend);
  providerKeys.setBackend(backend);
  customProviders.setBackend(backend);
  sessions.setBackend(backend);

  const storage = new AppStorage(
    settings,
    providerKeys,
    sessions,
    customProviders,
    backend,
  );
  setAppStorage(storage);

  const agent = new Agent({
    initialState: {
      systemPrompt: PAGES_SYSTEM_PROMPT,
      // Sonnet 4.5 is a reasonable default — fast enough for the iterative
      // chat loop and capable on the JSX-authoring task. Users can switch
      // via the model selector in the UI.
      model: getModel("anthropic", "claude-sonnet-4-5-20250929"),
      thinkingLevel: "off",
      messages: [],
      tools: defaultPagesTools(),
    },
    convertToLlm: defaultConvertToLlm,
  });

  const chatPanel = new ChatPanel();
  await chatPanel.setAgent(agent, {
    onApiKeyRequired: (provider: string) => ApiKeyPromptDialog.prompt(provider),
  });

  return { agent, chatPanel };
}
