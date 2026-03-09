<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, nextTick } from 'vue';
import { Send, Cpu, Loader2, ChevronDown, ChevronRight, Check } from 'lucide-vue-next';
import { marked } from 'marked';
import type { Message } from '../App.vue';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from '@/components/ui/collapsible';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';

const props = defineProps<{
  sessionId: string;
  messages: Message[];
}>();

const emit = defineEmits<{
  (e: 'update-messages', messages: Message[]): void;
}>();

// Refs
const chatBoxRef = ref<HTMLElement | null>(null);
const inputRef = ref<HTMLInputElement | null>(null);
const inputValue = ref('');

// Local state
const localMessages = ref<Message[]>([]);
const isConnected = ref(false);
const isThinking = ref(false);
const ws = ref<WebSocket | null>(null);

// UI State
const expandedTools = ref<Record<string, boolean>>({});
const expandedThinking = ref<Record<string, boolean>>({});

// Initialize
onMounted(() => {
  localMessages.value = JSON.parse(JSON.stringify(props.messages));
  connect();
  scrollToBottom();
});

onUnmounted(() => {
  if (ws.value) {
    ws.value.close();
  }
});

// Reconnect string changed
watch(() => props.sessionId, () => {
  localMessages.value = JSON.parse(JSON.stringify(props.messages));
  connect();
});

const connect = () => {
  if (ws.value) {
    ws.value.close();
  }
  
  const wsUrl = `ws://localhost:3000/ws?user_id=${encodeURIComponent(props.sessionId)}`;
  ws.value = new WebSocket(wsUrl);
  
  ws.value.onopen = () => {
    isConnected.value = true;
  };
  
  ws.value.onmessage = (event) => {
    handleMessage(event.data);
  };
  
  ws.value.onclose = () => {
    isConnected.value = false;
    isThinking.value = false;
    // Optional: auto reconnect logic
    // setTimeout(connect, 3000);
  };
  
  ws.value.onerror = (error) => {
    console.error('WebSocket error:', error);
    isConnected.value = false;
    isThinking.value = false;
  };
};

// WebSocket Handlers
const getLatestBotMessage = () => {
  const lastMsg = localMessages.value[localMessages.value.length - 1];
  if (lastMsg && lastMsg.role === 'bot') {
    return lastMsg;
  }
  
  const newBotMsg: Message = {
    id: Date.now().toString(),
    role: 'bot',
    content: '',
    timestamp: Date.now()
  };
  localMessages.value.push(newBotMsg);
  return newBotMsg;
};

const handleMessage = (data: string) => {
  try {
    const msg = JSON.parse(data);
    const botMsg = getLatestBotMessage();
    
    switch (msg.type) {
      case 'thinking':
        isThinking.value = true;
        botMsg.thinking = (botMsg.thinking || '') + msg.content;
        break;
      case 'tool_start':
        isThinking.value = true;
        if (!botMsg.toolCalls) botMsg.toolCalls = [];
        botMsg.toolCalls.push({
          id: Date.now().toString(),
          name: msg.name,
          arguments: msg.arguments || '',
          status: 'running',
          result: null
        });
        expandedTools.value[botMsg.id + '_' + (botMsg.toolCalls.length - 1)] = true;
        break;
      case 'tool_end':
        isThinking.value = true;
        if (botMsg.toolCalls && botMsg.toolCalls.length > 0) {
          const activeTool = botMsg.toolCalls[botMsg.toolCalls.length - 1];
          activeTool.status = 'complete';
          activeTool.result = msg.output;
        }
        break;
      case 'content':
        isThinking.value = false;
        botMsg.content += msg.content;
        break;
      case 'done':
        isThinking.value = false;
        break;
      case 'text':
        isThinking.value = false;
        botMsg.content += msg.content;
        break;
    }
  } catch (e) {
    // legacy text fallback
    isThinking.value = false;
    const botMsg = getLatestBotMessage();
    botMsg.content += data;
  }
  
  emitMessages();
  scrollToBottom();
};

const emitMessages = () => {
  // Pass a clone back up
  emit('update-messages', JSON.parse(JSON.stringify(localMessages.value)));
};

const sendMessage = () => {
  if (!inputValue.value.trim() || !isConnected.value) return;
  
  const text = inputValue.value;
  inputValue.value = '';
  
  // Add user message to UI
  localMessages.value.push({
    id: Date.now().toString(),
    role: 'user',
    content: text,
    timestamp: Date.now()
  });
  
  emitMessages();
  
  // Send to socket
  if (ws.value?.readyState === WebSocket.OPEN) {
    ws.value.send(text);
  }
  
  scrollToBottom();
  
  nextTick(() => {
    inputRef.value?.focus();
  });
};

const scrollToBottom = async () => {
  await nextTick();
  if (chatBoxRef.value) {
    chatBoxRef.value.scrollTop = chatBoxRef.value.scrollHeight;
  }
};

const renderMarkdown = (text: string) => {
  if (!text) return '';
  return marked(text, { breaks: true, gfm: true });
};


</script>

<template>
  <div class="flex flex-col h-full w-full relative">
    <!-- Header -->
    <header class="py-4 px-6 bg-slate-800/80 border-b border-white/10 flex justify-end items-center">
      <div class="flex items-center bg-black/30 px-3 py-1.5 rounded-full border border-white/5">
        <div 
          class="w-2 h-2 rounded-full mr-2 transition-all shadow-[0_0_8px_rgba(239,68,68,0.6)] bg-red-500"
          :class="{
            'bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.6)]': isConnected && !isThinking,
            'bg-violet-500 shadow-[0_0_8px_rgba(139,92,246,0.6)] animate-pulse': isThinking
          }"
        ></div>
        <span class="text-xs font-medium text-slate-400">
          {{ isThinking ? 'Nanobot is thinking...' : (isConnected ? 'Connected' : 'Disconnected') }}
        </span>
      </div>
    </header>

    <!-- Messages -->
    <ScrollArea class="flex-1 p-6" ref="chatBoxRef">
      <div class="flex flex-col gap-6 max-w-4xl mx-auto w-full pb-4">
        <div 
          v-for="msg in localMessages" 
          :key="msg.id" 
          class="flex flex-col max-w-[85%] animate-in fade-in slide-in-from-bottom-2 duration-300"
          :class="msg.role === 'user' ? 'self-end' : (msg.role === 'system' ? 'self-center max-w-[95%]' : 'self-start')"
        >
          <div v-if="msg.role !== 'system'" class="text-xs text-slate-400 mb-1.5 mx-1" :class="msg.role === 'user' ? 'text-right' : ''">
            {{ msg.role === 'user' ? 'You' : 'Nanobot' }}
          </div>
          
          <div class="rounded-2xl relative break-words" :class="{
            'bg-gradient-to-br from-blue-500 to-blue-700 text-white p-3 px-4 rounded-br-sm shadow-lg shadow-blue-500/20': msg.role === 'user',
            'bg-slate-800/80 border border-white/10 p-4 rounded-bl-sm shadow-lg shadow-black/20': msg.role === 'bot',
            'bg-black/20 text-slate-400 py-1.5 px-3 text-xs rounded-xl text-center': msg.role === 'system'
          }">
            
            <!-- System message -->
            <div v-if="msg.role === 'system'">
              {{ msg.content }}
            </div>
            
            <!-- Bot message structure -->
            <template v-else-if="msg.role === 'bot'">
              <!-- Thinking block -->
              <Collapsible 
                v-if="msg.thinking" 
                v-model:open="expandedThinking[msg.id]"
                class="rounded-xl mb-3 overflow-hidden border bg-violet-500/5 border-violet-500/20"
              >
                <CollapsibleTrigger class="w-full p-2.5 px-3.5 flex justify-between items-center hover:bg-violet-500/10 transition-colors cursor-pointer select-none group">
                  <div class="flex items-center gap-2 text-sm font-medium text-violet-400">
                    <Loader2 v-if="localMessages[localMessages.length - 1].id === msg.id && isThinking && !msg.content" class="animate-spin w-4 h-4" />
                    <Cpu v-else class="w-4 h-4" />
                    <span>Thinking Process</span>
                  </div>
                  <ChevronDown v-if="expandedThinking[msg.id]" class="w-4 h-4 text-slate-400" />
                  <ChevronRight v-else class="w-4 h-4 text-slate-400" />
                </CollapsibleTrigger>
                <CollapsibleContent class="px-3.5 pb-3.5 pt-0">
                  <div class="text-sm text-slate-400 italic whitespace-pre-wrap leading-relaxed">{{ msg.thinking }}</div>
                </CollapsibleContent>
              </Collapsible>
              
              <!-- Tool calls block -->
              <template v-if="msg.toolCalls && msg.toolCalls.length > 0">
                <Collapsible 
                  v-for="(tool, index) in msg.toolCalls" 
                  :key="index"
                  v-model:open="expandedTools[msg.id + '_' + index]"
                  class="rounded-xl mb-3 overflow-hidden border bg-emerald-500/5 border-emerald-500/20"
                >
                  <CollapsibleTrigger class="w-full p-2.5 px-3.5 flex justify-between items-center hover:bg-emerald-500/10 transition-colors cursor-pointer select-none">
                    <div class="flex items-center gap-2 text-sm font-medium text-emerald-400">
                      <Loader2 v-if="tool.status === 'running'" class="animate-spin w-4 h-4" />
                      <Check v-else class="w-4 h-4" />
                      <span>Used Tool: <span class="font-mono bg-emerald-500/20 px-1.5 py-0.5 rounded text-xs text-emerald-300 ml-1">{{ tool.name }}</span></span>
                    </div>
                    <ChevronDown v-if="expandedTools[msg.id + '_' + index]" class="w-4 h-4 text-slate-400" />
                    <ChevronRight v-else class="w-4 h-4 text-slate-400" />
                  </CollapsibleTrigger>
                  <CollapsibleContent class="px-3.5 pb-3.5 pt-0">
                    <div class="text-xs font-semibold text-slate-400 mb-1.5 mt-1">Arguments:</div>
                    <pre class="bg-black/30 rounded-lg p-2.5 font-mono text-xs text-slate-200 overflow-x-auto whitespace-pre-wrap break-all border-l-2 border-blue-500 m-0"><code>{{ tool.arguments || '{}' }}</code></pre>
                    
                    <template v-if="tool.result">
                      <div class="text-xs font-semibold text-slate-400 mb-1.5 mt-3">Result:</div>
                      <pre class="bg-black/30 rounded-lg p-2.5 font-mono text-xs text-slate-200 overflow-x-auto whitespace-pre-wrap break-all border-l-2 border-amber-500 max-h-[300px] overflow-y-auto m-0"><code>{{ tool.result }}</code></pre>
                    </template>
                  </CollapsibleContent>
                </Collapsible>
              </template>
              
              <!-- Standard content block -->
              <div v-if="msg.content" class="prose prose-invert max-w-none text-sm leading-relaxed" v-html="renderMarkdown(msg.content)"></div>
            </template>
            
            <!-- User message -->
            <div v-else class="text-sm">
              {{ msg.content }}
            </div>
          </div>
        </div>
        
        <!-- Typing indicator if bot is generating -->
        <div v-if="isThinking && !localMessages[localMessages.length - 1]?.content" class="flex flex-col self-start animate-in fade-in slide-in-from-bottom-2">
          <div class="bg-slate-800/80 border border-white/10 p-4 px-5 rounded-2xl rounded-bl-sm shadow-lg flex items-center justify-center">
            <div class="flex gap-1.5">
              <span class="w-1.5 h-1.5 bg-blue-500 rounded-full animate-bounce [animation-delay:0s]"></span>
              <span class="w-1.5 h-1.5 bg-blue-500 rounded-full animate-bounce [animation-delay:-0.15s]"></span>
              <span class="w-1.5 h-1.5 bg-blue-500 rounded-full animate-bounce [animation-delay:-0.3s]"></span>
            </div>
          </div>
        </div>
      </div>
    </ScrollArea>

    <!-- Input Area -->
    <div class="p-6 pt-0 bg-transparent shrink-0">
      <div class="max-w-4xl mx-auto w-full relative">
        <div class="flex bg-slate-900/70 border border-white/10 rounded-2xl p-2 shadow-xl backdrop-blur-xl focus-within:border-blue-500/50 focus-within:ring-2 focus-within:ring-blue-500/20 transition-all">
          <Input 
            ref="inputRef"
            v-model="inputValue" 
            @keydown.enter="sendMessage"
            placeholder="Message Nanobot..." 
            :disabled="!isConnected"
            autofocus
            class="flex-1 border-0 bg-transparent shadow-none focus-visible:ring-0 text-slate-100 px-4 h-11"
          />
          <Button 
            @click="sendMessage" 
            :disabled="!inputValue.trim() || !isConnected"
            class="w-11 h-11 rounded-xl bg-blue-500 hover:bg-blue-400 text-white shrink-0 ml-2"
            size="icon"
          >
            <Send class="w-5 h-5" />
          </Button>
        </div>
        <div class="text-center text-xs text-slate-500 mt-3 font-medium">
          Powered by Nanobot-rs Web Gateway
        </div>
      </div>
    </div>
  </div>
</template>

<style>
/* Markdown specific styling overrides since Tailwind Typography plugin is not installed */
.prose p { margin-bottom: 0.75em; }
.prose p:last-child { margin-bottom: 0; }
.prose a { color: #60a5fa; text-decoration: none; }
.prose a:hover { text-decoration: underline; }
.prose code { background-color: rgba(0,0,0,0.3); padding: 0.2em 0.4em; border-radius: 4px; font-family: monospace; font-size: 0.9em; color: #e2e8f0; }
.prose pre { background-color: rgba(0,0,0,0.4); padding: 12px; border-radius: 8px; overflow-x: auto; margin: 0.75em 0; border: 1px solid rgba(255,255,255,0.1); }
.prose pre code { background-color: transparent; padding: 0; font-size: 0.9em; }
</style>
