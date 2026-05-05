import React, { useState } from "react";
import { commands, type AppPromptMapping } from "@/bindings";
import { Dropdown, SettingContainer } from "@/components/ui";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { useSettings } from "../../../hooks/useSettings";

export const AppPromptMappings: React.FC = React.memo(() => {
  const { getSetting, refreshSettings } = useSettings();
  const mappings = (getSetting("app_prompt_mappings") || []) as AppPromptMapping[];
  const actions = getSetting("post_process_actions") || [];

  const [draftPattern, setDraftPattern] = useState("");
  const [draftActionKey, setDraftActionKey] = useState<number | null>(null);

  const actionOptions = actions
    .slice()
    .sort((a, b) => a.key - b.key)
    .map((a) => ({ value: String(a.key), label: `${a.key} - ${a.name}` }));

  const persist = async (next: AppPromptMapping[]) => {
    await commands.setAppPromptMappings(next);
    await refreshSettings();
  };

  const handleAdd = async () => {
    const pattern = draftPattern.trim();
    if (!pattern || draftActionKey === null) return;
    const next = [
      ...mappings.filter((m) => m.pattern.toLowerCase() !== pattern.toLowerCase()),
      { pattern, action_key: draftActionKey },
    ];
    await persist(next);
    setDraftPattern("");
    setDraftActionKey(null);
  };

  const handleRemove = async (index: number) => {
    const next = mappings.filter((_, i) => i !== index);
    await persist(next);
  };

  const handleUpdateActionKey = async (index: number, action_key: number) => {
    const next = mappings.map((m, i) => (i === index ? { ...m, action_key } : m));
    await persist(next);
  };

  const handleLoadPresets = async () => {
    await commands.applyDefaultAppPresets();
    await refreshSettings();
  };

  return (
    <SettingContainer
      title="Auto app -> action"
      description="When you dictate, Parley detects the active app and auto-runs the matching post-process action. Pattern matches the window title (case-insensitive substring)."
      descriptionMode="tooltip"
      layout="stacked"
      grouped={true}
    >
      <div className="space-y-3">
        <div className="flex items-center justify-between p-2 rounded-md bg-blue-500/5 border border-blue-500/20">
          <span className="text-sm text-mid-gray">
            Quick start: load 5 standard actions (casual / email / code / doc / AI prompt) + mappings for common apps.
          </span>
          <Button onClick={handleLoadPresets} variant="primary" size="md">
            Load presets
          </Button>
        </div>

        {actions.length === 0 && (
          <div className="p-3 bg-mid-gray/5 rounded-md border border-mid-gray/20">
            <p className="text-sm text-mid-gray">
              Create at least one post-process action above before adding a mapping.
            </p>
          </div>
        )}

        {mappings.length > 0 && (
          <div className="space-y-1">
            {mappings.map((m, i) => (
              <div
                key={`${m.pattern}-${i}`}
                className="flex items-center gap-2 p-2 rounded-md bg-mid-gray/5 border border-mid-gray/20"
              >
                <span className="text-sm font-mono flex-1 truncate" title={m.pattern}>
                  {m.pattern}
                </span>
                <span className="text-xs text-mid-gray">-&gt;</span>
                <div className="w-48">
                  <Dropdown
                    selectedValue={String(m.action_key)}
                    options={actionOptions}
                    onSelect={(v) => handleUpdateActionKey(i, Number(v))}
                    placeholder="Action"
                  />
                </div>
                <button
                  onClick={() => handleRemove(i)}
                  className="text-xs text-mid-gray/60 hover:text-red-400 px-2"
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        )}

        {actions.length > 0 && (
          <div className="flex items-center gap-2">
            <Input
              type="text"
              value={draftPattern}
              onChange={(e) => setDraftPattern(e.target.value)}
              placeholder='e.g. "Slack", "Gmail", "Cursor"'
              variant="compact"
              className="flex-1"
            />
            <div className="w-48">
              <Dropdown
                selectedValue={draftActionKey === null ? null : String(draftActionKey)}
                options={actionOptions}
                onSelect={(v) => setDraftActionKey(Number(v))}
                placeholder="Pick action"
              />
            </div>
            <Button
              onClick={handleAdd}
              disabled={!draftPattern.trim() || draftActionKey === null}
              variant="primary"
              size="md"
            >
              Add
            </Button>
          </div>
        )}
      </div>
    </SettingContainer>
  );
});

AppPromptMappings.displayName = "AppPromptMappings";
