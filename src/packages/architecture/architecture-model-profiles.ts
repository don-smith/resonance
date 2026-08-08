import type { ModelProfile } from '@langchain/core/language_models/profile';

// OpenRouter documents this model at https://openrouter.ai/deepseek/deepseek-v4-pro.
// Add or replace entries here when the Architecture agent's model changes.
export const architectureModelProfiles: Readonly<Record<string, ModelProfile>> = {
  'deepseek/deepseek-v4-pro': {
    maxInputTokens: 1_048_576,
    maxOutputTokens: 393_216,
    imageInputs: false,
    imageUrlInputs: false,
    pdfInputs: false,
    audioInputs: false,
    videoInputs: false,
    imageToolMessage: false,
    pdfToolMessage: false,
    reasoningOutput: true,
    imageOutputs: false,
    audioOutputs: false,
    videoOutputs: false,
    toolCalling: true,
    toolChoice: true,
    structuredOutput: true,
  },
};

export function getArchitectureModelProfile(model: string): ModelProfile | undefined {
  return architectureModelProfiles[model];
}
